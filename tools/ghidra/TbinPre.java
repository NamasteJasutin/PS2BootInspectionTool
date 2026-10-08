// Pre-analysis setup for rom0:TBIN (the PS1-mode kernel) analysed in place at 0xBFC4B800.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;

public class TbinPre extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        mem.createUninitializedBlock("ram", toAddr(0x0L), 0x200000L, false);
        mem.createUninitializedBlock("kseg0", toAddr(0x80000000L), 0x200000L, false);
        mem.createUninitializedBlock("kseg1", toAddr(0xA0000000L), 0x200000L, false);
        mem.createUninitializedBlock("hw", toAddr(0x1F800000L), 0x10000L, false);
        mem.createUninitializedBlock("hw1", toAddr(0xBF800000L), 0x10000L, false);
        Address start = toAddr(0xBFC4B800L);
        addEntryPoint(start);
        createFunction(start, "tbin_entry");
    }
}
