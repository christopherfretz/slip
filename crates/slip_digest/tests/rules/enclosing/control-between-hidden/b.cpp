namespace __attribute__((visibility("hidden"))) h { }
namespace __attribute__((visibility("hidden"))) k { int f() { return 1; } }
int u() { return k::f(); }
