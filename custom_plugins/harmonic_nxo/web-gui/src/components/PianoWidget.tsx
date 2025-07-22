import { forwardRef, useRef, type RefObject } from "react"
import {Div, type DivProps} from "style-props-html"

import useMonitorSize from "../utils/useMonitorSize"

export interface PianoWidgetProps extends DivProps {
    /** An array, filled with booleans, of length 128 (0-127), indicating which keys are currently held down */
    midiStates: Array<boolean>
}

const WHITE_KEY_ASPECT_RATIO = 1.0 / 8.0

export default forwardRef<HTMLDivElement, PianoWidgetProps>(function PianoWidget({midiStates,...rest}, ref) {
    const innerRef = useRef<HTMLDivElement|null>(null)
    const populatedRef = ref || innerRef
    const {width:containerWidth } = useMonitorSize(populatedRef as RefObject<HTMLDivElement|null>)?? { width: 0, height: 0 }
    const whiteKeyWidth = containerWidth / 88
    const whiteKeyHeight = whiteKeyWidth / WHITE_KEY_ASPECT_RATIO

    return <Div width="100%" height={`${whiteKeyHeight}px`} ref={populatedRef} {...rest}></Div>
})