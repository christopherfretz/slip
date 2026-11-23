namespace a { namespace b { } }
namespace __attribute__((visibility("hidden"))) a { namespace b { int f() { return 1; } } }
namespace m { }
int u() { return a::b::f(); }
