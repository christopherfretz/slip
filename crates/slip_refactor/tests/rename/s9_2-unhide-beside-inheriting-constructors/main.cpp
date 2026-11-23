namespace n {
struct B { B(int); virtual void f(); void f(int); };
struct D : B { using B::B; D(int); using B::f; void f() override; };
}
void h(n::D& d) { d.f(); d.f(1); }
