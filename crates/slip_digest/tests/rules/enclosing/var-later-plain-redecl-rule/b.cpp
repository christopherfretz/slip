namespace __attribute__((visibility("hidden"))) h { }
namespace h { }
namespace __attribute__((visibility("hidden"))) d { extern int v; }
int u() { return d::v; }
