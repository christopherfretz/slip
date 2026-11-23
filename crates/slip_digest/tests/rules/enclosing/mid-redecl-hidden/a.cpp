namespace h { int f(); }
namespace __attribute__((visibility("hidden"))) h { int f(); }
int u() { return h::f(); }
