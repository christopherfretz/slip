namespace __attribute__((visibility("hidden"))) h { int f(); }
namespace h { int f() { return 1; } }
namespace __attribute__((visibility("hidden"))) h { }
int u() { return h::f(); }
