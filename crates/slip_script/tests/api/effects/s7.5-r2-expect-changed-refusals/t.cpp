#define CALL g(1) + 0
struct S {};
int g(int);
int v = CALL;
S a, b;
struct F {
  friend S make();
};
void h() { extern S ext; }
static_assert(sizeof(S) == 1);
int k = g(2);
