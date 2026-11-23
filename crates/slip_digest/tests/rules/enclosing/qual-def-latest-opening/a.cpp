namespace n { int f(); }
namespace __attribute__((visibility("hidden"))) n { }
namespace m { }
int n::f() { return 1; }
int u() { return n::f(); }
