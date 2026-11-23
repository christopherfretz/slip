#define OVERRIDE_F void f() override
namespace n {
struct B { virtual void f(); };
struct D : B { OVERRIDE_F; };
}
