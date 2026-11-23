namespace __attribute__((visibility("hidden"))) h { extern int v; }
namespace h { extern int v; }
namespace __attribute__((visibility("hidden"))) d { }
int u() { return h::v; }
