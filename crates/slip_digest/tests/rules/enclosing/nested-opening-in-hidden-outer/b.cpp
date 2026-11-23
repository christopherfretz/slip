namespace a { namespace b { } }
namespace __attribute__((visibility("hidden"))) a { namespace b { } }
namespace m { int f() { return 1; } }
int u() { return m::f(); }
