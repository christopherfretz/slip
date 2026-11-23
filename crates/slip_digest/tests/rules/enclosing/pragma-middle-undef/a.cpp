namespace n { int f(); }
#pragma GCC visibility push(hidden)
namespace n { int f(); }
#pragma GCC visibility pop
int u() { return n::f(); }
