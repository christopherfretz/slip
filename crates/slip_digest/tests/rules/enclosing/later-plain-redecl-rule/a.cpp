namespace __attribute__((visibility("hidden"))) h { int f(); }
namespace h { int f(); }
namespace __attribute__((visibility("hidden"))) d { }
int u() { return h::f(); }
