namespace m { struct f {}; }
namespace n { template <class T> struct S { void f(); }; }
class X { friend m::f; };
