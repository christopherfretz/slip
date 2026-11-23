namespace __attribute__((visibility("hidden"))) h { extern "C" int cf() { return 1; } }
namespace d { }
int u() { return h::cf(); }
