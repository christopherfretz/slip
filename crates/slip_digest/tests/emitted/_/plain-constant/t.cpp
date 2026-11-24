#include "side.h"
// WP-155 fix round (B2, rules 2 and 3): initialisers that are plainly
// constant (spec/digest.md §11 "The mark"): no variable is marked, static
// or thread-local, and a static data member of a class template
// instantiated in side A only is not marked either.
enum E { e1, e2 };
void fn();
int a = 0;
long l = 1;
double b = -1.5;
float fl = 2;
bool c = true;
char ch = 'c';
const char *str = "x";
int *p = &a;
int *np = nullptr;
int *zp = 0;
int &ref = a;
const long &cref = l;
void (*fp)() = fn;
void (*afp)() = &fn;
void (&fr)() = fn;
E e = e2;
unsigned long sz = sizeof(int) + 0 * 0;
unsigned long sz2 = sizeof(double);
unsigned long al = alignof(long);
int br{1};
int em{};
int pr = (2);
int neg = -(+3);
bool fb = 1.0;
thread_local int tl = 0;
thread_local int *tp = &a;
template <class T> struct Tr { static int r; static thread_local int t; };
template <class T> int Tr<T>::r = 0;
template <class T> thread_local int Tr<T>::t = 0;
#ifndef B
int *pr2 = &Tr<int>::r;
int *pt2 = &Tr<int>::t;
#endif
