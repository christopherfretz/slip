namespace n {
struct B { virtual void f(int); void f(double); };
struct D : B { void f(int) override; };
struct E : B { using B::f; };
}
void h(n::E& e) { e.f(1); }
