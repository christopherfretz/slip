namespace a { int f() { return sizeof(__func__); } }
int u() { return a::f(); }
