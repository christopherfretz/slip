namespace n {
template <class T> struct B { virtual void f(); };
struct D : B<int> { void f() override; };
}
void h(n::D& d, n::B<int>& b) { d.f(); b.f(); }
