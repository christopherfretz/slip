namespace n {
struct B { virtual void f(); void f(int); };
template <class T> struct D : B { using B::f; void f() override; };
}
void h(n::D<int>& d) { d.f(); d.f(1); }
