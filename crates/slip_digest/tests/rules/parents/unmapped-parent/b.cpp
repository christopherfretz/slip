namespace x { namespace y { int c; int f() { return c; } } }
int use() { return x::y::c + x::y::f(); }
