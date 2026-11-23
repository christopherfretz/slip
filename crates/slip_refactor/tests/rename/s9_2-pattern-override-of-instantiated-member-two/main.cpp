namespace n {
template <class T> struct B { virtual void f(); };
struct D : B<int> { void f() override; };
struct E : B<long> { void f() override; };
}
void h(n::D& d, n::E& e) { d.f(); e.f(); }
