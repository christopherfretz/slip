#pragma redefine_extname f impl_f
extern "C" int f(int);

int caller() { return f(1); }
