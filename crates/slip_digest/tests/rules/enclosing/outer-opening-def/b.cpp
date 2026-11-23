namespace __attribute__((visibility("hidden"))) o { namespace i { } }
namespace o { }
namespace __attribute__((visibility("hidden"))) m { int f(); }
namespace m { int f() { return 1; } }
int u() { return m::f(); }
