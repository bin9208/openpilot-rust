/*
  Simple DirectMedia Layer
  Copyright (C) 1997-2024 Sam Lantinga <slouken@libsdl.org>

  This software is provided 'as-is', without any express or implied
  warranty.  In no event will the authors be held liable for any damages
  arising from the use of this software.

  Permission is granted to anyone to use this software for any purpose,
  including commercial applications, and to alter it and redistribute it
  freely, subject to the following restrictions:

  1. The origin of this software must not be misrepresented; you must not
     claim that you wrote the original software. If you use this software
     in a product, an acknowledgment in the product documentation would be
     appreciated but is not required.
  2. Altered source versions must be plainly marked as such, and must not be
     misrepresented as being the original software.
  3. This notice may not be removed or altered from any source distribution.
*/
// Altered ABI-only subset of SDL2 2.30.0 public headers; see abi-provenance.json.
#pragma once
#include <cstdint>
using Uint8=std::uint8_t; using Uint16=std::uint16_t; using Uint32=std::uint32_t; using Sint32=std::int32_t;
using SDL_Scancode=int; using SDL_Keycode=Sint32;
typedef struct SDL_Keysym
{
    SDL_Scancode scancode;
    SDL_Keycode sym;
    Uint16 mod;
    Uint32 unused;
} SDL_Keysym;
typedef struct SDL_WindowEvent
{
    Uint32 type;
    Uint32 timestamp;
    Uint32 windowID;
    Uint8 event;
    Uint8 padding1;
    Uint8 padding2;
    Uint8 padding3;
    Sint32 data1;
    Sint32 data2;
} SDL_WindowEvent;
typedef struct SDL_KeyboardEvent
{
    Uint32 type;
    Uint32 timestamp;
    Uint32 windowID;
    Uint8 state;
    Uint8 repeat;
    Uint8 padding2;
    Uint8 padding3;
    SDL_Keysym keysym;
} SDL_KeyboardEvent;
typedef struct SDL_MouseMotionEvent
{
    Uint32 type;
    Uint32 timestamp;
    Uint32 windowID;
    Uint32 which;
    Uint32 state;
    Sint32 x;
    Sint32 y;
    Sint32 xrel;
    Sint32 yrel;
} SDL_MouseMotionEvent;
typedef struct SDL_MouseButtonEvent
{
    Uint32 type;
    Uint32 timestamp;
    Uint32 windowID;
    Uint32 which;
    Uint8 button;
    Uint8 state;
    Uint8 clicks;
    Uint8 padding1;
    Sint32 x;
    Sint32 y;
} SDL_MouseButtonEvent;
typedef struct SDL_Rect
{
    int x, y;
    int w, h;
} SDL_Rect;
typedef struct SDL_Color
{
    Uint8 r;
    Uint8 g;
    Uint8 b;
    Uint8 a;
} SDL_Color;
union alignas(8) SDL_Event { Uint32 type; SDL_WindowEvent window; SDL_KeyboardEvent key; SDL_MouseMotionEvent motion; SDL_MouseButtonEvent button; Uint8 padding[56]; };
struct SDL_Window; struct SDL_Surface; struct SDL_Renderer; struct TTF_Font; struct Tcl_Interp; struct Tcl_Obj;
static_assert(sizeof(void*)==8); static_assert(sizeof(SDL_Event)==56); static_assert(sizeof(SDL_Keysym)==16); static_assert(sizeof(SDL_KeyboardEvent)==32);
