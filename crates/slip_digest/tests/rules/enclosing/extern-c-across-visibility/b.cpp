namespace __attribute__((visibility("hidden"))) h { }
namespace d { extern "C" int cf() { return 1; } }
int u() { return d::cf(); }
