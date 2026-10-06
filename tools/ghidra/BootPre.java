// Generic pre-analysis setup for a raw EE image (RESET / KERNEL / EELOAD).
// args (any order, repeatable):
//   bss=START:END          add an uninitialised block [START, END)
//   gp=ADDR                pin $gp over the whole image
//   entry=ADDR[:name]      seed an entry point / function
//   file=PATH:ADDR[:name]  add an initialised block from a file (e.g. the ROM at 0xBFC00000)
//   label=ADDR:name        plain label
// All numbers are hex with or without 0x.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.lang.Register;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.SourceType;
import java.io.File;
import java.io.FileInputStream;
import java.math.BigInteger;

public class BootPre extends GhidraScript {
    static long hex(String s) {
        s = s.trim();
        if (s.startsWith("0x") || s.startsWith("0X")) s = s.substring(2);
        return Long.parseLong(s, 16);
    }

    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        int n = 0;
        for (String arg : getScriptArgs()) {
            int eq = arg.indexOf('=');
            if (eq < 0) continue;
            String key = arg.substring(0, eq), val = arg.substring(eq + 1);
            String[] p = val.split(":");
            try {
            if (key.equals("bss")) {
                long s = hex(p[0]), e = hex(p[1]);
                mem.createUninitializedBlock(p.length > 2 ? p[2] : ".bss" + (n++), toAddr(s), e - s, false);
            } else if (key.equals("gp")) {
                Register gp = currentProgram.getRegister("gp");
                for (MemoryBlock b : mem.getBlocks()) {
                    if (!b.isInitialized()) continue;
                    currentProgram.getProgramContext().setValue(gp, b.getStart(), b.getEnd(), BigInteger.valueOf(hex(p[0])));
                }
                currentProgram.getSymbolTable().createLabel(toAddr(hex(p[0])), "_gp", SourceType.USER_DEFINED);
            } else if (key.equals("entry")) {
                Address a = toAddr(hex(p[0]));
                addEntryPoint(a);
                disassemble(a);
                createFunction(a, p.length > 1 ? p[1] : null);
                if (p.length > 1) currentProgram.getSymbolTable().createLabel(a, p[1], SourceType.USER_DEFINED);
            } else if (key.equals("file")) {
                File f = new File(p[0]);
                try (FileInputStream in = new FileInputStream(f)) {
                    MemoryBlock b = mem.createInitializedBlock(p.length > 2 ? p[2] : f.getName(), toAddr(hex(p[1])), in, f.length(), monitor, false);
                    b.setRead(true); b.setExecute(true); b.setWrite(false);
                }
            } else if (key.equals("label")) {
                currentProgram.getSymbolTable().createLabel(toAddr(hex(p[0])), p[1], SourceType.USER_DEFINED);
            }
            } catch (Exception e) {
                printerr("BootPre: " + arg + ": " + e);
            }
        }
    }
}
