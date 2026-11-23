namespace __attribute__((visibility("hidden"))) h { int f() { return 1; } }
namespace h { }
int u() { return h::f(); }
