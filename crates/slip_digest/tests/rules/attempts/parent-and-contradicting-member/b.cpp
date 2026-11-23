namespace b {}
namespace x { int f() { return 1; } }
int use() { return x::f(); }
