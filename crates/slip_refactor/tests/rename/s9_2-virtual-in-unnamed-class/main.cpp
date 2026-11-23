namespace n { struct B { virtual void f(); }; }
struct O { struct : n::B { void f() override {} } m, *p; };
