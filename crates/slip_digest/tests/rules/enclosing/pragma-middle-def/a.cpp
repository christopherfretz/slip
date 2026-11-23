namespace n { int f(); }
#pragma GCC visibility push(hidden)
namespace n { int f(); }
#pragma GCC visibility pop
namespace n { int f() { return 1; } }
