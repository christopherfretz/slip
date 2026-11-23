namespace __attribute__((visibility("hidden"))) h { }
namespace h { int f() { return 1; } }
int u() { return h::f(); }
