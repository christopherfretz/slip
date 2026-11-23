namespace { namespace n { int g() { extern int f(); return f(); } int f() { return 1; } } }
int u() { return n::g(); }
