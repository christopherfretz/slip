namespace a::b { void f(int); }
namespace b { void f(int); }
namespace a { namespace c { void f(int); } }
void h() { a::b::f(1); b::f(2); a::c::f(3); }
