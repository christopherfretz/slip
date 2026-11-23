namespace __attribute__((visibility("hidden"))) h { int f() { return 1; } }
namespace d { }
int u() { return h::f(); }
