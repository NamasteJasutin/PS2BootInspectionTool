// Applies a symbol table to the current program.
// args: <symbols.tsv> [more.tsv ...]
// TSV format: address<TAB>name[<TAB>note]; '#' lines and a header line starting with
// "address" are ignored. A function at the address is renamed (created first if the address
// is in executable memory and no function exists there); other addresses get a label.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.Symbol;
import java.io.BufferedReader;
import java.io.FileReader;
import java.util.HashSet;
import java.util.Set;

public class ApplySymbols extends GhidraScript {
    @Override
    public void run() throws Exception {
        int renamed = 0, created = 0, labels = 0, skipped = 0;
        Set<String> used = new HashSet<>();
        for (String path : getScriptArgs()) {
            try (BufferedReader r = new BufferedReader(new FileReader(path))) {
                String line;
                while ((line = r.readLine()) != null) {
                    line = line.trim();
                    if (line.isEmpty() || line.startsWith("#") || line.toLowerCase().startsWith("address")) continue;
                    String[] f = line.split("\t");
                    if (f.length < 2) continue;
                    String as = f[0].trim();
                    if (as.startsWith("0x") || as.startsWith("0X")) as = as.substring(2);
                    long av;
                    try { av = Long.parseLong(as, 16); } catch (NumberFormatException e) { skipped++; continue; }
                    String name = f[1].trim().replaceAll("[^A-Za-z0-9_$.]", "_");
                    if (name.isEmpty()) { skipped++; continue; }
                    if (!used.add(name)) {
                        // keep names unique so the export files stay unambiguous
                        name = name + "_" + as;
                    }
                    Address a = toAddr(av);
                    MemoryBlock b = currentProgram.getMemory().getBlock(a);
                    if (b == null) { skipped++; continue; }
                    Function fn = getFunctionAt(a);
                    if (fn == null && b.isExecute() && b.isInitialized()) {
                        if (getFunctionContaining(a) == null) {
                            disassemble(a);
                            fn = createFunction(a, name);
                            if (fn != null) created++;
                        }
                    }
                    if (fn != null) {
                        fn.setName(name, SourceType.USER_DEFINED);
                        renamed++;
                    } else {
                        Symbol s = getSymbolAt(a);
                        if (s != null && s.getSource() == SourceType.USER_DEFINED) continue;
                        createLabel(a, name, true, SourceType.USER_DEFINED);
                        labels++;
                    }
                }
            }
        }
        println("ApplySymbols: " + renamed + " functions named (" + created + " created), " + labels + " labels, " + skipped + " skipped");
    }
}
