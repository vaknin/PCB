import java.io.*;
import java.lang.reflect.*;
import java.util.*;

/** Load a DSN with Freerouting's own reader and list every clearance violation. */
public class Probe {
    static Object call(Object o, String m) throws Exception {
        Method mm = o.getClass().getMethod(m);
        mm.setAccessible(true);
        return mm.invoke(o);
    }
    static Object field(Object o, String f) throws Exception {
        Field ff = o.getClass().getField(f);
        return ff.get(o);
    }
    static String describe(Object it) throws Exception {
        String kind = it.getClass().getSimpleName();
        Object comp = call(it, "componentName");
        int[] nets = (int[]) field(it, "netNumbers");
        Object bb = call(it, "boundingBox");
        Object ll = field(bb, "ll"), ur = field(bb, "ur");
        double x = ((int) field(ll, "x") + (int) field(ur, "x")) / 2.0 / 10000.0;
        double y = -((int) field(ll, "y") + (int) field(ur, "y")) / 2.0 / 10000.0;
        String extra = "";
        try { extra = " " + call(it, "name"); } catch (Exception e) {}
        try { extra += " pin" + call(it, "get_padstack"); } catch (Exception e) {}
        return String.format("%s%s comp=%s nets=%s at(%.3f,%.3f)mm", kind, extra, comp, Arrays.toString(nets), x, y);
    }
    public static void main(String[] a) throws Exception {
        ClassLoader cl = Probe.class.getClassLoader();
        Class<?> obsC = Class.forName("app.freerouting.board.state.BoardObservers");
        Class<?> idC = Class.forName("app.freerouting.datastructures.IdGenerator");
        Object obs = Class.forName("app.freerouting.board.state.BoardObserverAdaptor").getConstructor().newInstance();
        int[] ctr = {0};
        Object id = Proxy.newProxyInstance(cl, new Class<?>[] {idC}, (p, m, args) -> m.getName().equals("newId") ? ++ctr[0] : ctr[0]);
        Method rb = Class.forName("app.freerouting.io.specctra.DsnReader").getMethod("readBoard", InputStream.class, obsC, idC);
        Object res = rb.invoke(null, new FileInputStream(a[0]), obs, id);
        System.out.println("read: " + res.getClass().getSimpleName());
        Object board = call(res, "board");
        Collection<?> items = (Collection<?>) call(board, "getItems");
        int n = 0;
        Set<String> seen = new TreeSet<>();
        for (Object it : items) {
            Collection<?> vs = (Collection<?>) call(it, "clearanceViolations");
            for (Object v : vs) {
                String s1 = describe(field(v, "firstItem")), s2 = describe(field(v, "secondItem"));
                String key = s1.compareTo(s2) < 0 ? s1 + " | " + s2 : s2 + " | " + s1;
                key = String.format("layer %s expected %.1f actual %.1f :: %s", field(v, "layer"), (double) field(v, "expectedClearance"), (double) field(v, "actualClearance"), key);
                if (seen.add(key)) n++;
            }
        }
        for (String s : seen) System.out.println(s);
        if (a.length > 1) {
            for (Object it : items) {
                if (!it.getClass().getSimpleName().equals("Pin")) continue;
                Object c = call(it, "componentName");
                if (c != null && Arrays.asList(a[1].split(",")).contains(c.toString()))
                    System.out.println("PIN " + describe(it) + " layers " + call(it, "firstLayer") + "-" + call(it, "lastLayer"));
            }
        }
        System.out.println("distinct violations: " + n + " (items " + items.size() + ")");
        System.exit(0);
    }
}
