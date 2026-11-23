namespace a {}
namespace b { int f() { return 1; } }
int use() { return b::f(); }
