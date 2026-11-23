#pragma GCC visibility push(hidden)
namespace h { int f() { return 1; } }
#pragma GCC visibility pop
namespace d { }
int u() { return h::f(); }
