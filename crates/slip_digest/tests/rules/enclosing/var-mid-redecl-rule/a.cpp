namespace h { extern int v; }
namespace __attribute__((visibility("hidden"))) h { extern int v; }
namespace d { }
namespace d { }
int u() { return h::v; }
