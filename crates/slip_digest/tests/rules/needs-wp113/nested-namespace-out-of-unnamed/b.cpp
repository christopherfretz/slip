namespace a { namespace { } namespace b { inline int f() { return 1; } } }
int u() { return a::b::f(); }
