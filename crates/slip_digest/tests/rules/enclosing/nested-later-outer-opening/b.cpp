namespace a { namespace b { int f(); } }
namespace a { }
int a::b::f() { return 1; }
