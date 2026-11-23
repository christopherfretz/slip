namespace n {
struct B { virtual void f(); };
struct D : B { void f() final; };
void D::f() {}
void B::f() {}
}
