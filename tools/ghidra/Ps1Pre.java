// Pre-analysis setup for the unpacked rom0:LOGO image (PS1 shell) loaded raw at 0x80030000:
// adds the PS1 kernel area, hardware registers and .bss, seeds the entry point and names the
// three kernel call vectors (A0/B0/C0) with their function-number register t1 as a parameter.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.data.DataType;
import ghidra.program.model.data.IntegerDataType;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.ParameterImpl;
import ghidra.program.model.listing.VariableStorage;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.SourceType;

public class Ps1Pre extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long imgEnd = 0x80030000L + currentProgram.getMemory().getBlocks()[0].getSize();
        mem.createUninitializedBlock(".bss", toAddr(imgEnd), 0x80200000L - imgEnd, false);
        mem.createUninitializedBlock("kernel", toAddr(0x0L), 0x10000L, false);
        mem.createUninitializedBlock("kseg0_kernel", toAddr(0x80000000L), 0x10000L, false);
        mem.createUninitializedBlock("hw", toAddr(0x1F800000L), 0x10000L, false);
        mem.createUninitializedBlock("bios", toAddr(0xBFC00000L), 0x80000L, false);
        Address start = toAddr(0x80030000L);
        addEntryPoint(start);
        createFunction(start, "shell_entry");
        Register t1 = currentProgram.getRegister("t1");
        for (long v : new long[] {0xA0L, 0xB0L, 0xC0L}) {
            Address a = toAddr(v);
            Function f = createFunction(a, "bios_" + Long.toHexString(v).toUpperCase());
            if (f == null) continue;
            f.setCustomVariableStorage(true);
            DataType dt = IntegerDataType.dataType;
            ParameterImpl p = new ParameterImpl("fn", dt, new VariableStorage(currentProgram, t1), currentProgram);
            f.addParameter(p, SourceType.USER_DEFINED);
        }
    }
}
