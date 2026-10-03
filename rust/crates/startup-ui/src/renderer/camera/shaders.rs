pub(super) const VERTEX: &str = r#"#version 300 es
precision mediump float;
in vec3 vertexPosition;
in vec2 vertexTexCoord;
in vec3 vertexNormal;
in vec4 vertexColor;
uniform mat4 mvp;
out vec2 fragTexCoord;
out vec4 fragColor;
void main() {
  fragTexCoord = vertexTexCoord;
  fragColor = vertexColor;
  gl_Position = mvp * vec4(vertexPosition, 1.0);
}
"#;
pub(super) fn fragment(compact: bool, external: bool) -> String {
    let mut code = "#version 300 es\n".to_owned();
    if external {
        code.push_str("#extension GL_OES_EGL_image_external_essl3 : enable\n");
    }
    code.push_str("precision mediump float;\nin vec2 fragTexCoord;\nout vec4 fragColor;\n");
    code.push_str(if external {
        "uniform samplerExternalOES texture0;\n"
    } else {
        "uniform sampler2D texture0;\nuniform sampler2D texture1;\n"
    });
    if compact {
        code.push_str("uniform int engaged;\nuniform int enhance_driver;\n");
    }
    code.push_str("void main() {\n");
    if external {
        code.push_str("vec4 color = texture(texture0, fragTexCoord);\n");
    } else {
        code.push_str("float y = texture(texture0, fragTexCoord).r;\nvec2 uv = texture(texture1, fragTexCoord).ra - 0.5;\nvec4 color = vec4(y + 1.402*uv.y, y - 0.344*uv.x - 0.714*uv.y, y + 1.772*uv.x, 1.0);\n");
    }
    if compact {
        code.push_str("if (engaged == 1) {\nfloat gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));\ncolor.rgb = mix(vec3(gray), color.rgb, 0.2);\ncolor.rgb = clamp((color.rgb - 0.5) * 1.2 + 0.5, 0.0, 1.0);\n");
        if external {
            code.push_str("color.rgb = pow(color.rgb, vec3(1.0/1.28));\n");
        }
        code.push_str("} else { color.rgb *= 0.85; }\nif (enhance_driver == 1) {\nfloat brightness = 1.1;\ncolor.rgb = color.rgb + 0.15;\ncolor.rgb = clamp((color.rgb - 0.5) * (brightness * 0.8) + 0.5, 0.0, 1.0);\ncolor.rgb = color.rgb * color.rgb * (3.0 - 2.0 * color.rgb);\ncolor.rgb = pow(color.rgb, vec3(0.8));\n}\n");
    } else if external {
        code.push_str("color.rgb = pow(color.rgb, vec3(1.0/1.28));\n");
    }
    code.push_str("fragColor = color;\n}\n");
    code
}
