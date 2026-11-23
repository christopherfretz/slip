namespace n {
struct A { virtual void f(); };
struct B : A { void f() override; template <class T> void f(T, int); };
struct D : B { using B::f; template <class T> void f(T, int); };
}
void h(n::D& d) { d.f(); d.f(1, 1); }
