namespace acme {
struct B { void f(); void f(int); };
struct D : B { using B::f; };
int g();
namespace in { using acme::g; }
}
void use(acme::D d) { d.f(1); }
