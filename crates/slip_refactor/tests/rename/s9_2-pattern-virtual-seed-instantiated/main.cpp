namespace n {
template <class T> struct B { virtual void f(); };
struct D : B<int> { void f() override; };
}
void h(n::B<int>& b) { b.f(); }
