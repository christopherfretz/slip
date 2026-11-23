namespace n { }
namespace __attribute__((visibility("hidden"))) n { }
namespace m { int f(); }
int m::f() { return 1; }
int u() { return m::f(); }
