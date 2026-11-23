namespace { }
struct S { friend int f(S) { return 1; } };
int u() { return f(S()); }
