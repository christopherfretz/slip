namespace __attribute__((visibility("hidden"))) o { namespace i { int f(); } }
namespace o { int i::f() { return 1; } }
namespace __attribute__((visibility("hidden"))) m { }
namespace m { }
int u() { return o::i::f(); }
