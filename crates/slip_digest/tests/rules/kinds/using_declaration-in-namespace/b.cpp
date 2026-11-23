namespace legacy {
struct B { void f(); void f(int); };
struct D : B { using B::f; };
int g();
namespace in { using legacy::g; }
}
void use(legacy::D d) { d.f(1); }
