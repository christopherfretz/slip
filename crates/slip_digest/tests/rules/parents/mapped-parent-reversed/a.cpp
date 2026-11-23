namespace a { namespace b { int c; int f() { return c; } } }
int use() { return a::b::c + a::b::f(); }
