namespace h { }
namespace __attribute__((visibility("hidden"))) h { }
namespace d { extern int v; }
namespace d { extern int v; }
int u() { return d::v; }
