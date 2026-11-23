namespace __attribute__((visibility("hidden"))) n { int f(); }
namespace n { }
namespace __attribute__((visibility("hidden"))) m { }
int n::f() { return 1; }
int u() { return n::f(); }
