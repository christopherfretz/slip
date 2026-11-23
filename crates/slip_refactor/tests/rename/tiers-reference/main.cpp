namespace n { void foo(int); }
#define ARG(x) x(1)
#define BODY n::foo(2)
#define CAT(a, b) a##b
void h() {
  n::foo(0);
  ARG(n::foo);
  BODY;
  n::CAT(f, oo)(3);
}
