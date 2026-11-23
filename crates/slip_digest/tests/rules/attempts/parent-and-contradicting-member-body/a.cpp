namespace a { int k = 1; }
namespace b { int f() { return a::k; } }
int use() { return b::f(); }
