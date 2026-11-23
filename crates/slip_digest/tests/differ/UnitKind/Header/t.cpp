// module: m=m.cppm
// §10.7 UnitKind `Header`: a function of a header included in the global module fragment of `m` (side A) against owned by the header unit `m` imports (side B). The main file imports `m` on both sides, so the order unit, which would be indeterminate for an import of a header unit (order.rs `HEADER_IMPORT`), is the same.
import m;
