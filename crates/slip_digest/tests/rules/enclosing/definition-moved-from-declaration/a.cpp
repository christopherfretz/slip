namespace __attribute__((visibility("hidden"))) h { int f(); }
namespace h { }
namespace __attribute__((visibility("hidden"))) h { int f() { return 1; } }
int u() { return h::f(); }
