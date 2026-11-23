namespace __attribute__((visibility("hidden"))) h { }
namespace h { }
namespace __attribute__((visibility("hidden"))) d { int f(); }
int u() { return d::f(); }
