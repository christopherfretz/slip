namespace __attribute__((visibility("hidden"))) h { int f(); }
namespace h { int f(); }
int u() { return h::f(); }
