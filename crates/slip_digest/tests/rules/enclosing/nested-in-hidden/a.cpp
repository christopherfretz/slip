namespace __attribute__((visibility("hidden"))) o { namespace i { int f() { return 1; } } }
namespace d { }
int u() { return o::i::f(); }
