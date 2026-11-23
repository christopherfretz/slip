#define V(a) virtual void a##f()
namespace n { struct B { virtual void f(); }; struct X { V(); }; void X::f() {} }
