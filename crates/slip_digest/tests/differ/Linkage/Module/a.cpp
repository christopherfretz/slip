// §5.6 Linkage `Module` (§1.7 row `enclosing`): a function of a named module, not exported, has module linkage.
export module m;
export int f() { return 1; }
