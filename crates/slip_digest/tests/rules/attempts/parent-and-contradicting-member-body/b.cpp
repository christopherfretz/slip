namespace b {}
namespace x { int k = 1; int f() { return k; } }
int use() { return x::f(); }
