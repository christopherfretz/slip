namespace h { int f(); }
namespace __attribute__((visibility("hidden"))) h { int f(); }
namespace d { }
namespace d { }
int u() { return h::f(); }
