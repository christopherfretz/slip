namespace n {
struct B { virtual void f(); };
template <class T> struct D : B { void f() override; };
}
void h(n::D<int>& d) { d.f(); }
