namespace n {
struct B { virtual void f(int); };
struct D : B { virtual void f(double); };
}
void h(n::D& d) { d.f(1.0); }
