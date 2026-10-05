// Pre-analysis setup for the unpacked OSDSYS image loaded raw at 0x200000:
// adds the .bss block, pins $gp and seeds the entry point.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.lang.Register;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.SourceType;
import java.math.BigInteger;

public class OsdPre extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        // The EE language spec adds MMIO blocks, so the image bounds are given explicitly.
        Address imgStart = toAddr(0x200000L);
        Address imgEnd = toAddr(0x2C8F74L);
        long bssEnd = 0x416A30L;
        mem.createUninitializedBlock(".bss", imgEnd, bssEnd - imgEnd.getOffset(), false);
        Register gp = currentProgram.getRegister("gp");
        currentProgram.getProgramContext().setValue(gp, imgStart, imgEnd.subtract(1),
            BigInteger.valueOf(0x2CFFF0L));
        Address start = toAddr(0x200008L);
        addEntryPoint(start);
        createFunction(start, "_start");
        Address main = toAddr(0x209EB8L);
        disassemble(main);
        createFunction(main, "main");
        currentProgram.getSymbolTable().createLabel(toAddr(0x2CFFF0L), "_gp", SourceType.USER_DEFINED);
    }
}
