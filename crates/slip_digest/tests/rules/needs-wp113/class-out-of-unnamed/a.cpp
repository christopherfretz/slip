namespace { struct Impl { int f() { return 1; } }; }
int u() { return Impl().f(); }
