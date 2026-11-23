#pragma GCC visibility push(hidden)
namespace h { }
#pragma GCC visibility pop
namespace d { int f() { return 1; } }
int u() { return d::f(); }
