namespace h { }
namespace __attribute__((visibility("hidden"))) h { }
namespace d { int f(); }
namespace d { int f(); }
int u() { return d::f(); }
