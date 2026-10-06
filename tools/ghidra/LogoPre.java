// Pre-analysis setup for the unpacked PS2LOGO image loaded raw at 0x100000:
// adds the .bss block, pins $gp and seeds the entry point / main.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.lang.Register;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.SourceType;
import java.math.BigInteger;

public class LogoPre extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        Address imgStart = toAddr(0x100000L);
        Address imgEnd = toAddr(0x130B6CL);
        long bssEnd = 0x1E02CCL;
        mem.createUninitializedBlock(".bss", toAddr(0x130B80L), bssEnd - 0x130B80L, false);
        Register gp = currentProgram.getRegister("gp");
        currentProgram.getProgramContext().setValue(gp, imgStart, imgEnd.subtract(1),
            BigInteger.valueOf(0x1389F0L));
        Address start = toAddr(0x100008L);
        addEntryPoint(start);
        createFunction(start, "_start");
        Address main = toAddr(0x102040L);
        disassemble(main);
        createFunction(main, "main");
        currentProgram.getSymbolTable().createLabel(toAddr(0x1389F0L), "_gp", SourceType.USER_DEFINED);
    }
}
