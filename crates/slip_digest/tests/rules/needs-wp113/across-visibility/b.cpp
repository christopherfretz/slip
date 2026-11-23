namespace __attribute__((visibility("hidden"))) h { }
namespace d { int f() { return 1; } }
int u() { return d::f(); }
