import type { ThemeProps } from "@rjsf/core";
import {
  TextWidget,
  PasswordWidget,
  TextareaWidget,
  SelectWidget,
  CheckboxWidget,
  RangeWidget,
  NumberWidget,
} from "./widgets";
import {
  FieldTemplate,
  ObjectFieldTemplate,
  ArrayFieldTemplate,
  ErrorListTemplate,
} from "./templates";

const drasiTheme: ThemeProps = {
  widgets: {
    TextWidget,
    PasswordWidget,
    TextareaWidget,
    SelectWidget,
    CheckboxWidget,
    RangeWidget,
    NumberWidget,
  },
  templates: {
    FieldTemplate,
    ObjectFieldTemplate,
    ArrayFieldTemplate,
    ErrorListTemplate,
  },
};

export default drasiTheme;

export {
  TextWidget,
  PasswordWidget,
  TextareaWidget,
  SelectWidget,
  CheckboxWidget,
  RangeWidget,
  NumberWidget,
  FieldTemplate,
  ObjectFieldTemplate,
  ArrayFieldTemplate,
  ErrorListTemplate,
};
