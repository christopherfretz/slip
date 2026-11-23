namespace __attribute__((visibility("hidden"))) h { int f() { return 1; } }
namespace __attribute__((visibility("hidden"))) k { }
int u() { return h::f(); }
