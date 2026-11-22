// §10.7 ModuleTable `points` is read from the first declaration (§2.3): a redeclaration that repeats `export` changes nothing (WP-033 review, item 13).
export module m;
export int f();
export int f() { return 1; }
