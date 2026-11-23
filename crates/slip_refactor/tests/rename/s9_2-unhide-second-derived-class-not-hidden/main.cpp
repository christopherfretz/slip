namespace n {
struct B { virtual void f(); void f(int); };
struct D : B { using B::f; void f() override; };
struct E : B { using B::f; };
}
void h(n::E& e) { e.f(); e.f(1); }
