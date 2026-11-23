namespace a { namespace b { int f(); } }
namespace __attribute__((visibility("hidden"))) a { namespace b { } }
int a::b::f() { return 1; }
