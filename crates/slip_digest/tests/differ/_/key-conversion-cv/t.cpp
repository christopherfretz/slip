#include "side.h"
// digest §3.4, WP-039 item 1: conversion functions differing in cv: two keys.
struct S { operator int();
#ifdef B
operator int() const;
#endif
};
