// Dumps decompiled C, a function index with call graph, and string xrefs.
// args: <output dir>
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.symbol.Reference;
import java.io.File;
import java.io.PrintWriter;
import java.util.Set;
import java.util.TreeSet;

public class OsdExport extends GhidraScript {
    @Override
    public void run() throws Exception {
        File out = new File(getScriptArgs()[0]);
        new File(out, "c").mkdirs();
        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        PrintWriter idx = new PrintWriter(new File(out, "functions.tsv"));
        idx.println("addr\tname\tsize\tcallers\tcallees");
        FunctionIterator it = currentProgram.getFunctionManager().getFunctions(true);
        while (it.hasNext() && !monitor.isCancelled()) {
            Function f = it.next();
            Set<String> callers = new TreeSet<>(), callees = new TreeSet<>();
            for (Function c : f.getCallingFunctions(monitor)) callers.add(c.getEntryPoint().toString());
            for (Function c : f.getCalledFunctions(monitor)) callees.add(c.getEntryPoint().toString());
            idx.println(f.getEntryPoint() + "\t" + f.getName() + "\t" + f.getBody().getNumAddresses()
                + "\t" + String.join(",", callers) + "\t" + String.join(",", callees));
            DecompileResults r = dec.decompileFunction(f, 120, monitor);
            PrintWriter w = new PrintWriter(new File(out, "c/" + f.getEntryPoint() + ".c"));
            if (r.decompileCompleted()) w.print(r.getDecompiledFunction().getC());
            else w.println("// decompile failed: " + r.getErrorMessage());
            w.close();
        }
        idx.close();
        PrintWriter sx = new PrintWriter(new File(out, "data_xrefs.tsv"));
        for (Data d : currentProgram.getListing().getDefinedData(true)) {
            StringBuilder sb = new StringBuilder();
            for (Reference ref : getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(ref.getFromAddress());
                sb.append(ref.getFromAddress()).append(f != null ? "(" + f.getEntryPoint() + ")" : "").append(' ');
            }
            if (sb.length() == 0) continue;
            String v = d.hasStringValue() ? String.valueOf(d.getValue()).replace("\n", "\\n") : d.getDataType().getName();
            sx.println(d.getAddress() + "\t" + v + "\t" + sb.toString().trim());
        }
        sx.close();
    }
}
